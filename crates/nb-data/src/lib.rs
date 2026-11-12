//! Données statiques de Norrblock : blocs, particules, mobs, réglages, packs de ressources.
//!
//! Les formats JSON (`blocks.json`, `particles.json`, `mobs.json`, `settings.json`) sont
//! ceux de la version Java. Les registres sont des structures immuables passées par référence
//! (aucun singleton global).

pub mod behavior;
pub mod blocks;
pub mod items;
pub mod keys;
pub mod mobs;
pub mod pack;
pub mod particles;
pub mod recipes;
pub mod settings;
pub mod spawn_rules;
pub mod survival;
pub mod universe;

pub use blocks::{
    BlockId, BlockIds, BlockPalette, BlockProperties, BlockRegistry, BlockType, PaletteReport,
    BLOCKS_JSON, CRAFT_BLOCKS_JSON, UNASSIGNED_ID,
};
pub use behavior::{BehaviorBook, Profile, StateKind, Stimulus};
pub use spawn_rules::{Avoid, Persistence, Sky, SpawnRules};
pub use universe::WorldKind;
pub use items::{ItemId, ItemInfo, ItemRegistry, FIRST_PURE_ITEM, ITEMS_JSON, STACK_MAX};
pub use mobs::{
    builtin_mob_ids, Gait, MobBehaviorType, MobConfig, MobGeometry, MobMedium, MobRegistry,
    BUILTIN_MOB_IDS,
};
pub use pack::PackMeta;
pub use particles::{ParticleRegistry, ParticleType, PARTICLES_JSON};
pub use recipes::{Ingredient, Output, Recipe, RecipeBook, Shape, Station, RECIPES_JSON};
pub use settings::{Action, GameSettings, ShadowQuality};
pub use survival::{
    ArmorInfo, ArmorSlot, BlockRule, CropRule, Drop, FoodInfo, ItemProps, Mining, SurvivalData,
    ToolInfo, ToolKind, SURVIVAL_JSON, TIER_NAMES,
};
