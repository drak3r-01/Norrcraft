//! Instantané des entrées du joueur produit par nb-game à chaque tick (spec §19.3).

use nb_core::glam::Vec2;

/// Actions de jeu lues par `PlayerInputSystem`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum GameAction {
    Forward,
    Backward,
    Left,
    Right,
    Jump,
    Sprint,
    Crouch,
    Primary,
    Secondary,
    Drop,
}

/// Ensemble d'actions (bitset).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ActionSet(u16);

impl ActionSet {
    pub const EMPTY: ActionSet = ActionSet(0);

    pub fn contains(self, a: GameAction) -> bool {
        self.0 & (1 << a as u8) != 0
    }

    pub fn insert(&mut self, a: GameAction) {
        self.0 |= 1 << a as u8;
    }

    pub fn remove(&mut self, a: GameAction) {
        self.0 &= !(1 << a as u8);
    }

    /// Version chaînable de [`ActionSet::insert`].
    pub fn with(mut self, a: GameAction) -> Self {
        self.insert(a);
        self
    }
}

impl FromIterator<GameAction> for ActionSet {
    fn from_iter<I: IntoIterator<Item = GameAction>>(iter: I) -> Self {
        let mut s = ActionSet::EMPTY;
        for a in iter {
            s.insert(a);
        }
        s
    }
}

/// Entrées d'un tick. Pendant la saisie du chat, nb-game fournit `pressed`/`just_pressed` vides
/// et un delta souris nul.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlayerInput {
    pub pressed: ActionSet,
    pub just_pressed: ActionSet,
    /// Delta souris en pixels depuis le tick précédent (Y écran vers le bas).
    pub mouse_delta: Vec2,
    /// Case de barre rapide sélectionnée, `0..=8`.
    pub selected_slot: u8,
    /// `false` quand l'écran d'inventaire est ouvert.
    pub mouse_locked: bool,
    /// `settings.sensitivity` ; `None` ⇒ 0.1 (pas de réglages, défaut Java).
    pub sensitivity: Option<f32>,
    /// `settings.fov` en degrés ; `None` ⇒ 70°.
    pub fov_deg: Option<f32>,
}

impl Default for PlayerInput {
    fn default() -> Self {
        Self {
            pressed: ActionSet::EMPTY,
            just_pressed: ActionSet::EMPTY,
            mouse_delta: Vec2::ZERO,
            selected_slot: 0,
            mouse_locked: true,
            sensitivity: None,
            fov_deg: None,
        }
    }
}

impl PlayerInput {
    pub fn is_pressed(&self, a: GameAction) -> bool {
        self.pressed.contains(a)
    }

    pub fn is_just_pressed(&self, a: GameAction) -> bool {
        self.just_pressed.contains(a)
    }
}

/// Molette : `selected −= signum(scroll)` avec bouclage 0..=8 (géré par nb-game, fourni ici).
pub fn scroll_slot(selected: u8, scroll: f64) -> u8 {
    let mut s = i32::from(selected);
    if scroll > 0.0 {
        s -= 1;
    } else if scroll < 0.0 {
        s += 1;
    }
    if s < 0 {
        s = 8;
    }
    if s > 8 {
        s = 0;
    }
    s as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_set_bits() {
        let s: ActionSet = [GameAction::Jump, GameAction::Drop].into_iter().collect();
        assert!(s.contains(GameAction::Jump));
        assert!(s.contains(GameAction::Drop));
        assert!(!s.contains(GameAction::Forward));
        let mut t = s.with(GameAction::Forward);
        t.remove(GameAction::Jump);
        assert!(t.contains(GameAction::Forward) && !t.contains(GameAction::Jump));
    }

    #[test]
    fn scroll_wraps() {
        assert_eq!(scroll_slot(0, 1.0), 8);
        assert_eq!(scroll_slot(8, -1.0), 0);
        assert_eq!(scroll_slot(4, 0.0), 4);
        assert_eq!(scroll_slot(4, -2.5), 5);
    }
}
