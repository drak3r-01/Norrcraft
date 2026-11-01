//! nb-game — logique de jeu de Norrblock : boucle fixe, entrées, chat, machine à états, raccord
//! monde ↔ maillage, sauvegardes. Aucune fenêtre ni GPU : tout est testable en headless.
//!
//! Spécification : `docs/spec/nb-game.md`.

pub mod chat;
pub mod chunks;
pub mod fixed_step;
pub mod game;
pub mod input;
pub mod render;
pub mod session;
pub mod state;
pub mod ui;

pub use chat::{Chat, ChatColor, ChatMessage, ChatView};
pub use chunks::{WorldBiomes, WorldChunks};
pub use fixed_step::{FixedStep, TIME_STEP};
pub use game::{parse_seed, Game, GameOptions, PlatformRequest, Step};
pub use input::{Action, InputManager};
pub use render::{MobInstance, RenderView};
pub use session::{Registries, WorldSession};
pub use state::{GameState, HudState, SettingsTab};
pub use ui::{NewWorld, NullUi, StubUi, Ui, UiCommand, UiView};

pub use nb_ecs::GameMode;
pub use nb_mesh::MeshAction;
