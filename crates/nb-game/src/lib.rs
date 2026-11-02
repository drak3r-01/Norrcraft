//! nb-game — logique de jeu de Norrblock : boucle fixe, entrées, chat, machine à états, raccord
//! monde ↔ maillage, sauvegardes. Aucune fenêtre ni GPU : tout est testable en headless.
//!
//! Spécification : `docs/spec/nb-game.md`.
//!
//! Usage : la plateforme (nb-app) pousse les événements dans [`Game::input`], appelle
//! [`Game::update`] autant de fois que [`FixedStep::advance`] l'indique, puis applique
//! [`Game::take_requests`] / [`Game::take_mesh_actions`] et rend [`Game::render_view`].
//! L'interface est branchée par [`Game::set_ui`] (trait [`Ui`], implémenté par nb-ui).
//!
//! # Écarts volontaires par rapport au Java
//!
//! 1. Durée de frame bornée à 0,25 s avant l'accumulateur ([`FixedStep::java`] pour l'original).
//! 2. Chunks actifs triés par (x, z) avant le remaillage (le Java itère un `HashMap`).
//! 3. Changement de monde : génération de maillage incrémentée, aucun maillage en vol de
//!    l'ancien monde ne peut être affiché.
//! 4. Sauvegarde dont le `world.json` est illisible : la graine du monde est conservée (le Java
//!    repartait d'un `new WorldMetadata()`, graine 0).
//! 5. Bogue Java corrigé (décision de Gabriel) : PAUSED → MAIN MENU décharge le monde
//!    ([`Game::unload_to_menu_world`]) ; le Java le laissait chargé et pouvait le resauvegarder
//!    sous un nom périmé (dossier d'un monde renommé ou supprimé recréé).

pub mod chat;
pub mod chunks;
pub mod fixed_step;
pub mod game;
pub mod input;
pub mod render;
pub mod screens;
pub mod session;
pub mod state;
pub mod ui;

pub use chat::{Chat, ChatColor, ChatMessage, ChatView};
pub use chunks::{WorldBiomes, WorldChunks};
pub use fixed_step::{FixedStep, TIME_STEP};
pub use game::{parse_seed, Game, GameOptions, PlatformRequest, Step};
pub use input::{Action, InputManager};
pub use render::{MobInstance, ParticleLayers, RenderView};
pub use screens::NbUi;
pub use session::{Registries, WorldSession};
pub use state::{GameState, HudState, SettingsTab};
pub use ui::{NewWorld, NullUi, StubUi, Ui, UiCommand, UiFrame, UiView};

pub use nb_ecs::GameMode;
pub use nb_mesh::MeshAction;
