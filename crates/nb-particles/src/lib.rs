//! Moteur de particules de Norrblock : pool à capacité fixe, simulation avec collisions
//! légères, effets déclaratifs, émetteurs d'ambiance, export d'instances GPU (32 octets).
//!
//! Pur CPU : le jeu fournit un [`ParticleWorld`], nb-gfx dessine les [`GpuParticle`] en un seul
//! appel instancié.

pub mod ambient;
pub mod effect;
pub mod gpu;
pub mod presets;
pub mod rng;
pub mod system;
pub mod world;

pub use ambient::{Ambient, AmbientCtx};
pub use effect::{behav, sprites, EffectDef, SPRITE_NAMES};
pub use gpu::{GpuParticle, FLAG_BLOCK_ATLAS, FLAG_EMISSIVE, FLAG_VERTICAL};
pub use presets::BlockLook;
pub use rng::Rng;
pub use system::{Emit, Particle, ParticleSystem, Stats, View};
pub use world::{Cell, EmptyWorld, ParticleWorld};
