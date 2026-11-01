//! Données statiques de Norrblock : blocs, particules, mobs, réglages, packs de ressources.
//!
//! Les formats JSON (`blocks.json`, `particles.json`, `mobs.json`, `settings.json`) sont
//! ceux de la version Java. Les registres sont des structures immuables passées par référence
//! (aucun singleton global).

pub mod blocks;
pub mod keys;
pub mod mobs;
pub mod pack;
pub mod particles;
pub mod settings;

pub use blocks::{BlockId, BlockIds, BlockProperties, BlockRegistry, BlockType, BLOCKS_JSON};
pub use mobs::{MobBehaviorType, MobConfig, MobGeometry, MobMedium, MobRegistry, BUILTIN_MOB_IDS};
pub use pack::PackMeta;
pub use particles::{ParticleRegistry, ParticleType, PARTICLES_JSON};
pub use settings::{Action, GameSettings, ShadowQuality};
