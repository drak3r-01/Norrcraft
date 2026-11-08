//! Socle commun de Norrblock : générateur `java.util.Random`, maths, positions de
//! chunks et erreurs partagées. Aucune dépendance GPU ni fenêtre.

pub mod error;
pub mod math;
pub mod random;
pub mod thread;

pub use error::{Error, Result};
pub use math::{BlockPos, ChunkPos};
pub use random::JavaRandom;

/// Re-export de `glam` pour que tous les crates partagent la même version des types.
pub use glam;
