//! États du jeu (`GameState.java`) et petites logiques d'état sans rendu.

use nb_data::BlockId;

/// `GameState`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GameState {
    MainMenu,
    WorldSelect,
    Playing,
    Paused,
    Inventory,
    Settings,
    ResourcePacks,
    MobSettings,
}

impl GameState {
    pub const ALL: [GameState; 8] = [
        GameState::MainMenu,
        GameState::WorldSelect,
        GameState::Playing,
        GameState::Paused,
        GameState::Inventory,
        GameState::Settings,
        GameState::ResourcePacks,
        GameState::MobSettings,
    ];

    /// L'état appelle `MasterRenderer.render` (monde 3D visible derrière l'UI).
    pub fn renders_world(self) -> bool {
        matches!(
            self,
            GameState::MainMenu | GameState::Playing | GameState::Paused | GameState::Inventory
        )
    }

    /// La surbrillance du bloc visé est dessinée (`UIManager.render`).
    pub fn shows_highlight(self) -> bool {
        matches!(
            self,
            GameState::Playing | GameState::Paused | GameState::Inventory
        )
    }

    /// Voile noir à 60 % sur la scène (PAUSED, INVENTORY).
    pub fn dims_world(self) -> bool {
        matches!(self, GameState::Paused | GameState::Inventory)
    }
}

/// Onglet initial de l'écran des réglages (`pendingSettingsTab`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum SettingsTab {
    #[default]
    Graphics,
    Controls,
}

/// Affichage du nom de l'objet tenu : 1 s plein, puis 0,5 s de fondu.
pub const ITEM_NAME_SHOW: f32 = 1.0;
pub const ITEM_NAME_FADE: f32 = 0.5;

/// État du HUD de `PlayingState` qui évolue par tick (minuterie du nom de l'objet tenu).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HudState {
    pub last_selected_slot: i32,
    pub last_held: Option<BlockId>,
    pub item_name_timer: f32,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            last_selected_slot: -1,
            last_held: None,
            item_name_timer: ITEM_NAME_SHOW + ITEM_NAME_FADE,
        }
    }
}

impl HudState {
    /// Fin de `PlayingState.update`.
    pub fn update(&mut self, selected_slot: u8, held: Option<BlockId>, dt: f64) {
        let slot = i32::from(selected_slot);
        if slot != self.last_selected_slot || held != self.last_held {
            self.last_selected_slot = slot;
            self.last_held = held;
            if held.is_some() {
                self.item_name_timer = 0.0;
            }
        }
        self.item_name_timer =
            (self.item_name_timer + dt as f32).min(ITEM_NAME_SHOW + ITEM_NAME_FADE);
    }

    /// `itemNameAlpha()`.
    pub fn item_name_alpha(&self) -> f32 {
        if self.item_name_timer >= ITEM_NAME_SHOW + ITEM_NAME_FADE {
            0.0
        } else if self.item_name_timer < ITEM_NAME_SHOW {
            1.0
        } else {
            1.0 - (self.item_name_timer - ITEM_NAME_SHOW) / ITEM_NAME_FADE
        }
    }
}

/// `PlayingState.formatBlockName` : `oak_leaves` → `Oak Leaves`.
pub fn format_block_name(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_timer() {
        let mut h = HudState::default();
        assert_eq!(h.item_name_alpha(), 0.0);
        h.update(0, Some(3), 0.0);
        assert_eq!(h.item_name_alpha(), 1.0);
        h.update(0, Some(3), 1.25);
        assert!((h.item_name_alpha() - 0.5).abs() < 1e-6);
        h.update(0, Some(3), 1.0);
        assert_eq!(h.item_name_alpha(), 0.0);
        // Case vide : la minuterie n'est pas relancée.
        h.update(1, None, 0.0);
        assert_eq!(h.item_name_alpha(), 0.0);
    }

    #[test]
    fn block_names() {
        assert_eq!(format_block_name("oak_leaves"), "Oak Leaves");
        assert_eq!(format_block_name("stone"), "Stone");
        assert_eq!(format_block_name("_a__b_"), "A B");
    }

    #[test]
    fn state_flags() {
        let world: Vec<_> = GameState::ALL
            .iter()
            .filter(|s| s.renders_world())
            .collect();
        assert_eq!(world.len(), 4);
        assert!(GameState::Inventory.shows_highlight());
        assert!(!GameState::MainMenu.shows_highlight());
    }
}
