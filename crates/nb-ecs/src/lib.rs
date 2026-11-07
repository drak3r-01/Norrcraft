//! nb-ecs — entités de Norrblock : composants, joueur, physique AABB, raycasts, inventaire,
//! mobs (spawn, IA passive/agressive, santé), particules et commandes du chat.
//!
//! Spécification : `docs/spec/nb-ecs.md`. Ordre réel des systèmes du Java (le code fait foi) :
//! PlayerInput, Physics, PassiveAI, AggressiveAI, MobSpawn, Health ([`Schedule::java_default`]).
//!
//! # Écarts volontaires par rapport au Java (décisions de Gabriel, 2026-11-01)
//!
//! 1. **Ordre d'itération des entités** : ids croissants (déterministe) au lieu de l'ordre d'un
//!    `HashMap<Integer>` Java (B1).
//! 2. **Nombres des commandes** : toujours un point décimal (`1.0`), quelle que soit la locale
//!    (B12).
//! 3. **Plantages Java remplacés par « rien »** : `nextInt(≤ 0)` dans le spawn
//!    (`mobSpawnRadius == 0`, `groupSizeMax < groupSizeMin`) → aucune apparition ; un agresseur
//!    dont la config a disparu est ignoré par la détection de fuite des passifs (le Java levait
//!    une `NullPointerException`).
//! 4. Le spawn du monde est un `Option<[f64; 3]>` (`WorldConfig::spawn`) au lieu du triplet NaN.
//!
//! Tout le reste (bizarreries B2–B23 de la spec) est reproduit tel quel.

pub mod ai;
pub mod blocks;
pub mod command;
pub mod components;
pub mod crafting;
pub mod health;
pub mod input;
pub mod inventory;
pub mod javafmt;
pub mod jmath;
pub mod particles;
pub mod physics;
pub mod player;
pub mod schedule;
pub mod sim;
pub mod spawn;
pub mod world;

pub use blocks::{BlockAccess, MemoryBlocks, WorldBlocks};
pub use command::{execute_command, TimeControl};
pub use components::{
    AggressiveAi, CameraState, Collider, EntityId, GameMode, Health, InventoryComp, MobType,
    PassiveAi, Player, Transform, Velocity,
};
pub use crafting::{Button, CraftingGrid, GRID_BASE, OUTPUT_SLOT};
pub use input::{ActionSet, GameAction, PlayerInput};
pub use inventory::{Inventory, InventoryCursor, ItemStack};
pub use particles::{Particle, ParticleManager};
pub use physics::{BlockEnv, RaycastHit};
pub use schedule::{EcsEvent, EcsRngs, Schedule, System, TickCtx};
pub use sim::{Simulation, TICK_DT};
pub use world::EntityWorld;

/// Re-export de `hecs` (requêtes génériques de [`EntityWorld`]).
pub use hecs;
