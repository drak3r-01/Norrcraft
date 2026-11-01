//! États du jeu (`GameState`) et contexte partagé par les écrans (`GameContext`, partie UI).

/// `com.engine.core.GameState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// Champs de `GameContext` lus / écrits par les écrans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiContext {
    /// `previousState` : retour des réglages.
    pub previous_state: GameState,
    /// `pendingSettingsTab == "CONTROLS"` : ouvrir les réglages sur l'onglet Contrôles.
    pub pending_controls_tab: bool,
}

impl Default for UiContext {
    fn default() -> Self {
        Self {
            previous_state: GameState::MainMenu,
            pending_controls_tab: false,
        }
    }
}
