//! Écrans de menu (états `MAIN_MENU`, `PAUSED`, `SETTINGS`, `RESOURCE_PACKS`, …).
//!
//! Chaque écran garde son état, lit un [`UiInput`](crate::UiInput) dans `update` et renvoie
//! une action que nb-game applique ; `draw` remplit une [`DrawList`](crate::DrawList).

pub mod common;
pub mod lan_overlay;
pub mod menus;
pub mod mobs;
pub mod multiplayer;
pub mod resource_packs;
pub mod settings;
pub mod world_select;

pub use lan_overlay::{draw_player_list, draw_terrain_loading, PlayerRow};
pub use menus::{LanPause, MainMenu, MenuAction, PauseMenu};
pub use mobs::{MobAction, MobSettingsScreen};
pub use multiplayer::{MultiplayerAction, MultiplayerScreen, MultiplayerView, ServerRow};
pub use resource_packs::{PackAction, PackEntry, ResourcePackScreen};
pub use settings::{SettingsAction, SettingsScreen, SettingsTab};
pub use world_select::{
    universe_label, WorldAction, WorldEntry, WorldGenParams, WorldSelectScreen, WorldType,
};
