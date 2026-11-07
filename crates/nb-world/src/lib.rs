//! nb-world — monde voxel de Norrblock : bruit, biomes, génération de terrain (identique au
//! Java pour une même graine), chunks, lumière, gestionnaire asynchrone et sauvegardes.
//!
//! Spécification : `docs/spec/nb-world.md`.
//!
//! # Écarts volontaires par rapport au Java (décisions de Gabriel, 2026-11-01)
//!
//! 1. **Monde FLAT sans nappe de lave** : la passe « lacs de lave » (remplissage de l'air sous
//!    y = 12) ne s'exécute que si les grottes sont actives — elle ne sert qu'à remplir le fond
//!    des grottes. Les mondes NORMAL / AMPLIFIED / ISLAND (grottes actives) sont identiques au
//!    Java bit à bit ; FLAT (grottes coupées) n'est plus noyé sous la lave. Le comportement
//!    d'origine reste disponible via [`WorldGenerator::with_java_lava_pass`] (tests).
//! 2. **`treeDensityOverrides`** : un tableau de longueur ≠ 8 lu dans un `world.json` est
//!    complété par `-1.0` / tronqué (le Java plantait la génération des chunks concernés).
//! 3. **Course lumière / `setBlock`** : un calcul de lumière commencé avant une écriture de bloc
//!    ne remet plus `light_dirty` à `false` (compteur d'éditions par chunk).
//! 4. **Sauvegardes** : nouveau format versionné (`world.json` avec `formatVersion`, chunks
//!    `.nbc` avec en-tête), sans compatibilité avec les mondes Java ; `player: None` remplace la
//!    sentinelle `playerY == 100.0` ; un nom assaini vide donne le répertoire `world`.
//! 5. `InMemoryChunkStorage` copie les voxels au lieu d'aliaser le tableau (sans effet observable).
//!
//! 6. **Générateur « Norrblock »** (2026-11-07, hors fidélité Java) : nouveau terrain par défaut,
//!    voir `docs/WORLDGEN.md` ; le générateur Java reste disponible (`GeneratorKind::Classic`).
//!
//! Les autres corrections retenues (shadow map, accumulateur 60 Hz, tri du quadtree) relèvent
//! d'autres crates.

pub mod biome;
pub mod chunk;
pub mod config;
mod flora;
pub mod generator;
mod generator_nb;
pub mod light;
pub mod manager;
pub mod noise;
pub mod save;
pub mod storage;
pub mod structures;
pub mod terrain;
pub mod world;

pub use biome::{
    Biome, BiomeConfig, BiomeSystem, FloraEntry, TreeType, VegetationEntry, BIOME_COUNT,
    CLASSIC_BIOME_COUNT, SOIL_ASH, SOIL_GRASS, SOIL_SAND, SOIL_SNOW,
};
pub use chunk::{
    fnv1a, java_index, Chunk, ChunkEntry, ChunkFlags, ChunkSnapshot, SharedChunk, CHUNK_SIZE_X,
    CHUNK_SIZE_Y, CHUNK_SIZE_Z, CHUNK_VOLUME,
};
pub use config::{GeneratorKind, OreConfig, OreRange, WorldConfig, WorldType};
pub use generator::WorldGenerator;
pub use light::{compute_light, LightContext, LightTable};
pub use manager::{BlockEdit, ChunkManager, DEFAULT_RENDER_DISTANCE};
pub use noise::{hash_position, java_string_hash, Noise};
pub use storage::{ChunkStorage, FileChunkStorage, InMemoryChunkStorage};
pub use structures::{StructureKind, StructureSite};
pub use terrain::{NbColumn, Terrain};
pub use world::{find_spawn_location, World};
