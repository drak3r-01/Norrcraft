//! nb-game — logique de jeu de Norrblock : boucle fixe, entrées, chat, machine à états, raccord
//! monde ↔ maillage, sauvegardes. Aucune fenêtre ni GPU : tout est testable en headless.
//!
//! Spécification : `docs/spec/nb-game.md`.

pub mod fixed_step;

pub use fixed_step::{FixedStep, TIME_STEP};
