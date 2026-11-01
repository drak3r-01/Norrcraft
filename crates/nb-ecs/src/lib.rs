//! nb-ecs — entités de Norrblock : composants, joueur, physique AABB, raycasts, inventaire,
//! mobs (spawn, IA, santé), particules et commandes du chat.
//!
//! Spécification : `docs/spec/nb-ecs.md`.

pub mod blocks;
pub mod javafmt;
pub mod jmath;

pub use blocks::{BlockAccess, MemoryBlocks, WorldBlocks};
