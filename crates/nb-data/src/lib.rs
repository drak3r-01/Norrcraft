//! Données statiques de Norrblock : blocs, particules, mobs, réglages, packs de ressources.
//!
//! Les formats JSON (`blocks.json`, `particles.json`, `mobs.json`, `settings.json`) sont
//! ceux de la version Java. Les registres sont des structures immuables passées par référence
//! (aucun singleton global).

pub mod blocks;
pub mod items;
pub mod keys;
pub mod mobs;
pub mod pack;
pub mod particles;
pub mod recipes;
pub mod settings;

pub use blocks::{
    BlockId, BlockIds, BlockProperties, BlockRegistry, BlockType, BLOCKS_JSON, CRAFT_BLOCKS_JSON,
};
pub use items::{ItemId, ItemInfo, ItemRegistry, FIRST_PURE_ITEM, ITEMS_JSON, STACK_MAX};
pub use mobs::{
    builtin_mob_ids, MobBehaviorType, MobConfig, MobGeometry, MobMedium, MobRegistry,
    BUILTIN_MOB_IDS,
};
pub use pack::PackMeta;
pub use particles::{ParticleRegistry, ParticleType, PARTICLES_JSON};
pub use recipes::{Ingredient, Output, Recipe, RecipeBook, Shape, Station, RECIPES_JSON};
pub use settings::{Action, GameSettings, ShadowQuality};
