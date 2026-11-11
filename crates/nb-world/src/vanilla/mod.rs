//! Univers « vanilla » : générateurs Classic (Java) et Norrblock, biomes, structures, flore.
//!
//! Code déplacé tel quel depuis la racine de la crate (plan Norrcraft, session 0.1) ; les noms
//! publics sont réexportés à l'identique par `lib.rs`.

pub mod biome;
pub mod config;
pub(crate) mod flora;
pub mod generator;
pub(crate) mod generator_nb;
pub mod structures;
pub mod terrain;
