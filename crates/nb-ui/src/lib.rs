//! nb-ui — interface 2D de Norrblock (voir `docs/spec/nb-ui.md`).
//!
//! Pur CPU : chaque écran produit une [`DrawList`] (quads colorés / texturés, glyphes
//! `stb_easy_font`, icônes de blocs) rendue par `nb_gfx::ui_pass`, et renvoie des actions que
//! nb-game applique (changement d'état, sauvegarde, réglages…).

pub mod chat;
pub mod draw;
pub mod font;
pub mod hud;
pub mod input;
pub mod inventory;
pub mod jfmt;
pub mod screens;
pub mod state;
pub mod textures;
pub mod widgets;

pub use chat::{Chat, ChatKeys, ChatMessage};
pub use draw::{Batch, DrawList, IconStyle, Rgba, Scissor, TextureRef, Vertex};
pub use hud::{Hud, HudView, SlotView};
pub use input::{TypingBuffers, UiInput};
pub use inventory::{ClickButton, InventoryAction, InventoryScreen};
pub use state::{GameState, UiContext};
pub use textures::UiTexture;
