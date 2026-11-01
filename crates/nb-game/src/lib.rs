//! nb-game — logique de jeu de Norrblock : boucle fixe, entrées, chat, machine à états, raccord
//! monde ↔ maillage, sauvegardes. Aucune fenêtre ni GPU : tout est testable en headless.
//!
//! Spécification : `docs/spec/nb-game.md`.

pub mod chat;
pub mod chunks;
pub mod fixed_step;
pub mod input;

pub use chat::{Chat, ChatColor, ChatMessage, ChatView};
pub use chunks::{WorldBiomes, WorldChunks};
pub use fixed_step::{FixedStep, TIME_STEP};
pub use input::{Action, InputManager};
