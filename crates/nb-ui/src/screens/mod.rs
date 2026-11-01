//! Écrans de menu (états `MAIN_MENU`, `PAUSED`, `SETTINGS`, `RESOURCE_PACKS`, …).
//!
//! Chaque écran garde son état, lit un [`UiInput`](crate::UiInput) dans `update` et renvoie
//! une action que nb-game applique ; `draw` remplit une [`DrawList`](crate::DrawList).

pub mod common;
pub mod menus;
pub mod resource_packs;
pub mod settings;
pub mod world_select;

pub use menus::{MainMenu, MenuAction, PauseMenu};
pub use resource_packs::{PackAction, PackEntry, ResourcePackScreen};
pub use settings::{SettingsAction, SettingsScreen, SettingsTab};
pub use world_select::{WorldAction, WorldEntry, WorldGenParams, WorldSelectScreen, WorldType};
